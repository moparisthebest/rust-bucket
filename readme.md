Rust Bucket
-----------

My ideal vision of a pastebin/image/file host, easy to use from the command line with any tools, and from a browser with and without javascript

Borrows ideas and inspiration from many different pastebins over the years:

  * [ZeroBin](https://github.com/sebsauvage/ZeroBin) (client-side encryption)
  * [chefi](https://github.com/colemickens/chefi) (paste with netcat)
  * [Rocket.rs example pastebin](https://github.com/SergioBenitez/Rocket/tree/master/examples/pastebin/src) (framework)
  * [dank-paste](https://github.com/wpbirney/dank-paste) (multipart/form-data support)
  * [ix.io](http://ix.io/) (downloadable client)
  
Delete after 1/2 views, 5/10 minutes, 1 hour/day/week/month/year, forever, delete after not viewed within X

Don't store file type, only used for link, can use any suffix for link

Store when to delete (how many views + how long), store how many times viewed, store when uploaded (or file creation date?)

Maybe if we only support 'burn after reading' like zerobin we can use a special created_date to signify this and then never count views, or, another file, since that would interfere with 'delete after X time'
  
Crazy random thoughts:
  * hash IDs, use un-hashed ID to encrypt paste
